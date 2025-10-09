use serde::{Deserialize, Serialize};

pub use soulseed_agi_core_models::{
    AIId, AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
    EnvelopeHead, EnvelopeId, EventId, EvidencePointer, GroupId, HumanId, MessageId, Provenance,
    SessionId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EnvelopeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EnvelopeId>,
}

fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolLane {
    ToolInvocation,
    SelfReflection,
    Collaboration,
}

impl ToolLane {
    fn from_scenario(scenario: Option<&ConversationScenario>) -> Self {
        match scenario {
            Some(ConversationScenario::AiSelfTalk) => Self::SelfReflection,
            Some(ConversationScenario::AiToAi)
            | Some(ConversationScenario::AiGroup)
            | Some(ConversationScenario::HumanToMultiAi)
            | Some(ConversationScenario::MultiHumanToMultiAi) => Self::Collaboration,
            _ => Self::ToolInvocation,
        }
    }

    fn from_scene(scene: &str) -> Option<Self> {
        let scene_lower = scene.to_ascii_lowercase();
        if scene_lower.contains("self") || scene_lower.contains("reflect") {
            return Some(Self::SelfReflection);
        }
        if scene_lower.contains("collab")
            || scene_lower.contains("multi_ai")
            || scene_lower.contains("coagent")
        {
            return Some(Self::Collaboration);
        }
        None
    }

    pub fn classify(anchor: &Anchor, scene: &str) -> Self {
        let lane = Self::from_scenario(anchor.scenario.as_ref());
        if matches!(lane, Self::ToolInvocation) {
            Self::from_scene(scene).unwrap_or(lane)
        } else {
            lane
        }
    }

    pub fn metric_tag(&self) -> &'static str {
        match self {
            Self::ToolInvocation => "tooling",
            Self::SelfReflection => "self_reflection",
            Self::Collaboration => "collaboration",
        }
    }

    pub fn extra_hints(&self) -> &'static [&'static str] {
        match self {
            Self::ToolInvocation => &[],
            Self::SelfReflection => &["self_reflection", "introspection"],
            Self::Collaboration => &["multi_agent", "collaboration"],
        }
    }

    pub fn adjust_risk_gate(&self, default_level: u8) -> u8 {
        match self {
            Self::SelfReflection => default_level.min(1),
            Self::Collaboration => default_level.max(2),
            Self::ToolInvocation => default_level,
        }
    }

    pub fn fallback_reason(&self) -> &'static str {
        match self {
            Self::ToolInvocation => "lane_tooling_generic",
            Self::SelfReflection => "lane_self_reflection_guard",
            Self::Collaboration => "lane_collaboration_coordination",
        }
    }
}

impl Anchor {
    pub fn lane(&self, scene: &str) -> ToolLane {
        ToolLane::classify(self, scene)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanLineage {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolPlan {
    pub plan_id: String,
    pub anchor: Anchor,
    pub schema_v: u16,
    pub lineage: PlanLineage,
    pub subject: Option<Subject>,
    pub items: Vec<ToolCallSpec>,
    pub strategy: OrchestrationStrategy,
    pub budget: Budget,
    pub explain_plan: ExplainPlan,
    pub lane: ToolLane,
    #[serde(default)]
    pub graph: ToolGraph,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolEdgeKind {
    Sequential,
    Dependency,
    Barrier,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolEdge {
    pub from: usize,
    pub to: usize,
    pub kind: ToolEdgeKind,
}

impl ToolEdge {
    pub fn sequential(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            kind: ToolEdgeKind::Sequential,
        }
    }

    pub fn barrier(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            kind: ToolEdgeKind::Barrier,
        }
    }
}

impl ToolGraph {
    pub fn linear(node_count: usize) -> Self {
        let mut edges = Vec::new();
        for idx in 1..node_count {
            edges.push(ToolEdge::sequential(idx - 1, idx));
        }
        Self {
            edges,
            barriers: Vec::new(),
        }
    }

    pub fn parallel(node_count: usize) -> Self {
        let _ = node_count; // unused but kept for parity
        Self::default()
    }

    pub fn with_barrier(node_count: usize) -> Self {
        let mut graph = Self::linear(node_count);
        if node_count > 1 {
            graph.barriers.push(ToolBarrier {
                barrier_id: String::new(),
                nodes: (0..node_count).collect(),
            });
        }
        graph
    }

    pub fn assign_barrier_ids(&mut self, plan_id: &str) {
        for (idx, barrier) in self.barriers.iter_mut().enumerate() {
            barrier.barrier_id = format!("{}::barrier-{}", plan_id, idx);
        }
    }

    pub fn topo_order(&self, node_count: usize) -> Vec<usize> {
        use std::collections::VecDeque;

        if node_count <= 1 {
            return (0..node_count).collect();
        }

        let mut indegree = vec![0usize; node_count];
        let mut adj = vec![Vec::new(); node_count];
        for edge in &self.edges {
            if edge.from < node_count && edge.to < node_count {
                indegree[edge.to] = indegree[edge.to].saturating_add(1);
                adj[edge.from].push(edge.to);
            }
        }

        let mut queue = VecDeque::new();
        for idx in 0..node_count {
            if indegree[idx] == 0 {
                queue.push_back(idx);
            }
        }

        let mut order = Vec::with_capacity(node_count);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &next in &adj[node] {
                indegree[next] = indegree[next].saturating_sub(1);
                if indegree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != node_count {
            return (0..node_count).collect();
        }
        order
    }

    pub fn barriers_containing(&self, node: usize) -> Vec<&ToolBarrier> {
        self.barriers
            .iter()
            .filter(|barrier| barrier.nodes.contains(&node))
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolBarrier {
    pub barrier_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolGraph {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<ToolEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barriers: Vec<ToolBarrier>,
}

impl Default for ToolGraph {
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            barriers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCallSpec {
    pub tool_id: String,
    pub schema_v: u16,
    pub input: serde_json::Value,
    pub params: serde_json::Value,
    pub cacheable: bool,
    pub stream: bool,
    pub idem_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrchestrationMode {
    Parallel,
    Serial,
    Hybrid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryBackoff {
    pub max_retries: u8,
    pub base_ms: u32,
    pub jitter: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CircuitBreaker {
    pub open_after: u32,
    pub half_open_after: u32,
    pub error_ratio: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FallbackRule {
    pub on: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationStrategy {
    pub mode: OrchestrationMode,
    pub retry: RetryBackoff,
    pub timeout_ms: u32,
    pub deadline_ms: Option<u64>,
    pub circuit: CircuitBreaker,
    pub fallback: Vec<FallbackRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DegradeRule {
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    pub max_cost_tokens: u64,
    pub max_latency_ms: u32,
    pub qos_policy: String,
    pub degrade_rules: Vec<DegradeRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolResultSummary {
    pub tool_id: String,
    pub schema_v: u16,
    pub summary: serde_json::Value,
    pub evidence_pointer: EvidencePointer,
    pub result_digest: String,
    #[serde(default)]
    pub lineage: SummaryLineage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct SummaryLineage {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CandidateScore {
    pub tool_id: String,
    pub score: f32,
    pub rank: u16,
    pub factors: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExplainPlan {
    pub schema_v: u16,
    pub candidates: Vec<CandidateScore>,
    pub excluded: Vec<(String, String)>,
    pub weights: serde_json::Value,
    pub policy_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunStage {
    pub tool_id: String,
    pub action: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub outcome: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExplainRun {
    pub schema_v: u16,
    pub stages: Vec<RunStage>,
    pub degradation_reason: Option<String>,
    pub indices_used: Option<Vec<String>>,
    pub query_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collected_summaries: Vec<ToolResultSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionRecord {
    pub summary: Option<ToolResultSummary>,
    pub explain_run: ExplainRun,
    pub fallback_triggered: bool,
    pub meta: PlanExecutionMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterRequest {
    pub anchor: Anchor,
    pub scene: String,
    pub capability_hints: Vec<String>,
    pub context_tags: serde_json::Value,
    pub subject: Option<Subject>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterOutput {
    pub ranked: Vec<CandidateScore>,
    pub excluded: Vec<(String, String)>,
    pub policy_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlannerContext {
    pub anchor: Anchor,
    pub ranked: Vec<CandidateScore>,
    pub policy_digest: Option<String>,
    pub excluded: Vec<(String, String)>,
    pub scene: String,
    pub capability_hints: Vec<String>,
    pub request_context: serde_json::Value,
    pub catalog: Vec<RouterCandidate>,
    pub subject: Option<Subject>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanBuild {
    pub plan: ToolPlan,
    pub explain_run: Option<ExplainRun>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EngineInput {
    pub anchor: Anchor,
    pub scene: String,
    pub capability_hints: Vec<String>,
    pub context_tags: serde_json::Value,
    pub request_context: serde_json::Value,
    pub subject: Option<Subject>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineOutput {
    pub plan: ToolPlan,
    pub execution: ToolExecutionRecord,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogue_events: Vec<DialogueEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolHistoryStats {
    pub success_rate: f32,
    pub latency_p95: f32,
    pub cost_per_success: f32,
    pub risk_level: String,
    pub auth_impact: f32,
    pub cacheable: bool,
    pub degradation_acceptance: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterCandidate {
    pub tool_id: String,
    pub version: String,
    pub capability: Vec<String>,
    pub side_effect: bool,
    pub supports_stream: bool,
    pub stats: ToolHistoryStats,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterState {
    pub candidates: Vec<RouterCandidate>,
    pub policy_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanExecutionMeta {
    pub degradation_reason: Option<String>,
    pub indices_used: Option<Vec<String>>,
    pub query_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_tool_degradation: Vec<(String, Option<String>)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OrchestratorResult {
    pub summary: Option<ToolResultSummary>,
    pub explain_run: ExplainRun,
    pub fallback_triggered: bool,
    pub meta: PlanExecutionMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCallAttempt {
    pub spec: ToolCallSpec,
    pub result: Result<ToolResultSummary, String>,
}

#[cfg(test)]
impl Default for RetryBackoff {
    fn default() -> Self {
        Self {
            max_retries: 1,
            base_ms: 100,
            jitter: true,
        }
    }
}

#[cfg(test)]
impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            open_after: 10,
            half_open_after: 1000,
            error_ratio: 0.5,
        }
    }
}

#[cfg(test)]
impl Default for OrchestrationStrategy {
    fn default() -> Self {
        Self {
            mode: OrchestrationMode::Serial,
            retry: RetryBackoff::default(),
            timeout_ms: 300,
            deadline_ms: None,
            circuit: CircuitBreaker::default(),
            fallback: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_cost_tokens: 10_000,
            max_latency_ms: 1_000,
            qos_policy: "default".into(),
            degrade_rules: Vec::new(),
        }
    }
}
