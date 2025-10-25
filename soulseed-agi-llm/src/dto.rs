use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
#[cfg(feature = "vectors-extra")]
use soulseed_agi_core_models::ExtraVectors;
use soulseed_agi_core_models::{
    ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType, EnvelopeHead, EventId,
    EvidencePointer, RealTimePriority, Snapshot, Subject, SubjectRef, TraceId,
};
use soulseed_agi_core_models::legacy::dialogue_event::DialogueEvent as LegacyDialogueEvent;
use soulseed_agi_tools::dto::{Anchor, ToolResultSummary};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptSegment {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptBundle {
    pub system: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation: Vec<PromptSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmPlanStep {
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct PlanLineage {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrivacyDirective {
    #[serde(default)]
    pub allow_sensitive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
}

impl Default for PrivacyDirective {
    fn default() -> Self {
        Self {
            allow_sensitive: false,
            ticket_id: None,
            approved_by: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmPlan {
    pub plan_id: String,
    pub anchor: Anchor,
    pub schema_v: u16,
    #[serde(default)]
    pub lineage: PlanLineage,
    pub steps: Vec<LlmPlanStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmInput {
    pub anchor: Anchor,
    pub schema_v: u16,
    #[serde(default)]
    pub lineage: PlanLineage,
    pub scene: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarify_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummary>,
    pub user_prompt: String,
    #[serde(default)]
    pub privacy: PrivacyDirective,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub context_tags: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degrade_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_indices: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_query_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelProfile {
    pub model_id: String,
    pub policy_digest: String,
    pub safety_tier: String,
    pub max_output_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_rank: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f32>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub should_use_factors: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelCandidate {
    pub profile: ModelProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusion_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostics: Value,
}

impl ModelCandidate {
    pub fn selected(profile: ModelProfile) -> Self {
        Self {
            profile,
            exclusion_reason: None,
            diagnostics: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRoutingDecision {
    pub selected: ModelProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<ModelCandidate>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub policy_trace: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningVisibility {
    SummaryOnly,
    Full,
    TicketRequired,
    Redacted,
}

impl Default for ReasoningVisibility {
    fn default() -> Self {
        ReasoningVisibility::SummaryOnly
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmResult {
    pub completion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub provider_metadata: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning: Vec<PromptSegment>,
    #[serde(default)]
    pub reasoning_visibility: ReasoningVisibility,
    #[serde(default)]
    pub redacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
    pub usage: TokenUsage,
    #[serde(default)]
    pub lineage: PlanLineage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmExplain {
    pub schema_v: u16,
    #[serde(default)]
    pub lineage: PlanLineage,
    pub anchor: Anchor,
    pub plan: LlmPlanExplain,
    pub run: LlmRunExplain,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmPlanExplain {
    pub selected: ModelProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<ModelCandidate>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub policy_trace: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmRunExplain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
    pub usage: TokenUsage,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub provider_metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmOutput {
    pub schema_v: u16,
    #[serde(default)]
    pub lineage: PlanLineage,
    pub final_event: DialogueEvent,
    pub plan: LlmPlan,
    pub model: ModelProfile,
    pub result: LlmResult,
    pub explain: LlmExplain,
}

pub fn build_final_event(
    anchor: &Anchor,
    scene: &str,
    result: &LlmResult,
    model: &ModelProfile,
    degrade_chain: Option<String>,
    indices_used: Option<Vec<String>>,
    query_hash: Option<String>,
) -> DialogueEvent {
    let scenario = anchor
        .scenario
        .clone()
        .unwrap_or(ConversationScenario::HumanToAi);
    let subject = Subject::AI(
        anchor
            .provenance
            .as_ref()
            .and_then(|prov| prov.model.clone())
            .map(|_| soulseed_agi_core_models::AIId::from_raw_unchecked(0))
            .unwrap_or_else(|| soulseed_agi_core_models::AIId::from_raw_unchecked(0)),
    );
    let participants = vec![SubjectRef {
        kind: Subject::Human(
            anchor
                .session_id
                .map(|id| soulseed_agi_core_models::HumanId::from_raw_unchecked(id.as_u64()))
                .unwrap_or_else(|| soulseed_agi_core_models::HumanId::from_raw_unchecked(0)),
        ),
        role: Some("user".into()),
    }];
    let envelope_head = EnvelopeHead {
        envelope_id: anchor.envelope_id,
        trace_id: TraceId(format!("llm:{}", anchor.envelope_id)),
        correlation_id: CorrelationId(model.model_id.clone()),
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
    };
    let should_use_factors = if !model.should_use_factors.is_null() {
        model.should_use_factors.clone()
    } else {
        json!({
            "source": "llm_engine",
            "reasoning_visibility": result.reasoning_visibility,
            "usage_tokens": {
                "prompt": result.usage.prompt_tokens,
                "completion": result.usage.completion_tokens,
            },
            "degradation_chain": degrade_chain
        })
    };
    let now = OffsetDateTime::now_utc();
    let final_event_legacy = LegacyDialogueEvent {
        tenant_id: anchor.tenant_id,
        event_id: EventId::from_raw_unchecked(now.unix_timestamp_nanos() as u64),
        session_id: anchor.session_id.expect("final event requires session"),
        subject,
        participants,
        head: envelope_head,
        snapshot: Snapshot {
            schema_v: anchor.schema_v,
            created_at: now,
        },
        timestamp_ms: (now.unix_timestamp_nanos() / 1_000_000) as i64,
        scenario,
        event_type: DialogueEventType::Message,
        time_window: None,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        sequence_number: anchor.sequence_number.unwrap_or(1) + 100,
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: Vec::new(),
        reasoning_trace: None,
        reasoning_confidence: None,
        reasoning_strategy: None,
        content_embedding: None,
        context_embedding: None,
        decision_embedding: None,
        embedding_meta: None,
        concept_vector: None,
        semantic_cluster_id: None,
        cluster_method: None,
        concept_distance_to_goal: None,
        real_time_priority: Some(RealTimePriority::Normal),
        notification_targets: None,
        live_stream_id: None,
        growth_stage: Some("llm:final".into()),
        processing_latency_ms: None,
        influence_score: None,
        community_impact: None,
        evidence_pointer: result.evidence_pointer.clone(),
        content_digest_sha256: Some(format!(
            "sha256:{}",
            blake3::hash(result.completion.as_bytes())
        )),
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(soulseed_agi_core_models::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId::generate(),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: {
            let mut meta = Map::new();
            meta.insert("scene".into(), json!(scene));
            meta.insert("model_id".into(), json!(model.model_id.clone()));
            meta.insert("policy_digest".into(), json!(model.policy_digest.clone()));
            meta.insert("degradation_chain".into(), json!(degrade_chain.clone()));
            meta.insert(
                "usage".into(),
                json!({
                    "prompt_tokens": result.usage.prompt_tokens,
                    "completion_tokens": result.usage.completion_tokens,
                    "total_cost_usd": result.usage.total_cost_usd,
                    "currency": result.usage.currency,
                }),
            );
            if let Some(summary) = result.summary.as_ref() {
                meta.insert("summary".into(), json!(summary));
            }
            if let Some(reason) = result.degradation_reason.as_ref() {
                meta.insert("degradation_reason".into(), json!(reason));
            }
            if let Some(indices) = indices_used.clone() {
                meta.insert("indices_used".into(), json!(indices));
            }
            if let Some(hash) = query_hash.clone() {
                meta.insert("query_hash".into(), json!(hash));
            }
            if let Some(rank) = model.selection_rank {
                meta.insert("model_rank".into(), json!(rank));
            }
            if let Some(score) = model.selection_score {
                meta.insert("model_score".into(), json!(score));
            }
            if let Some(band) = model.usage_band.as_ref() {
                meta.insert("usage_band".into(), json!(band));
            }
            if let Some(cost) = model.estimated_cost_usd {
                meta.insert("estimated_cost_usd".into(), json!(cost));
            } else if let Some(cost) = result.usage.total_cost_usd {
                meta.insert("estimated_cost_usd".into(), json!(cost));
            }
            meta.insert("should_use_factors".into(), should_use_factors);
            meta.insert(
                "reasoning_visibility".into(),
                json!(result.reasoning_visibility.clone()),
            );
            meta.insert("redacted".into(), json!(result.redacted));
            if !result.provider_metadata.is_null() {
                meta.insert("provider_metadata".into(), result.provider_metadata.clone());
            }
            Value::Object(meta)
        },
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    };
    let event = soulseed_agi_core_models::convert_legacy_dialogue_event(final_event_legacy);
    validate_event(&event);
    event
}

pub fn new_plan_id(_anchor: &Anchor) -> String {
    format!("llm-plan-{}", Uuid::now_v7())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmExecutionIntent {
    pub intent_id: String,
    pub anchor: Anchor,
    pub schema_v: u16,
    #[serde(default)]
    pub lineage: PlanLineage,
    pub model: ModelProfile,
    pub prompt: PromptBundle,
    #[serde(default)]
    pub privacy: PrivacyDirective,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degrade_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_indices: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_query_hash: Option<String>,
    pub issued_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub intent: LlmExecutionIntent,
    pub result: LlmResult,
    pub degradation_chain: Option<String>,
    pub indices_used: Option<Vec<String>>,
    pub query_hash: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlannedLlm {
    pub input: LlmInput,
    pub plan: LlmPlan,
    pub model_decision: ModelRoutingDecision,
    pub prompt: PromptBundle,
    pub intent: LlmExecutionIntent,
}

fn validate_event(event: &DialogueEvent) {
    if let Err(err) = soulseed_agi_core_models::validate_dialogue_event(event) {
        debug_assert!(false, "dialogue event validation failed: {:?}", err);
    }
}
