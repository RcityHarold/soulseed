use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use soulseed_agi_core_models::{
    ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType, EnvelopeHead, EventId,
    RealTimePriority, Snapshot, Subject, SubjectRef, TraceId,
};
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmPlan {
    pub plan_id: String,
    pub anchor: Anchor,
    pub steps: Vec<LlmPlanStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmInput {
    pub anchor: Anchor,
    pub scene: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarify_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummary>,
    pub user_prompt: String,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmResult {
    pub completion: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning: Vec<PromptSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
    pub usage: TokenUsage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmExplain {
    pub model_id: String,
    pub policy_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmOutput {
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
            .map(|_| soulseed_agi_core_models::AIId::new(0))
            .unwrap_or_else(|| soulseed_agi_core_models::AIId::new(0)),
    );
    let participants = vec![SubjectRef {
        kind: Subject::Human(
            anchor
                .session_id
                .map(|id| soulseed_agi_core_models::HumanId(id.0))
                .unwrap_or_else(|| soulseed_agi_core_models::HumanId(0)),
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
    let now = OffsetDateTime::now_utc();
    DialogueEvent {
        tenant_id: anchor.tenant_id,
        event_id: EventId(now.unix_timestamp_nanos() as u64),
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
        evidence_pointer: None,
        content_digest_sha256: Some(format!(
            "sha256:{}",
            blake3::hash(result.completion.as_bytes())
        )),
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(soulseed_agi_core_models::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId(now.unix_timestamp() as u64),
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
                }),
            );
            if let Some(reason) = result.degradation_reason.as_ref() {
                meta.insert("degradation_reason".into(), json!(reason));
            }
            if let Some(indices) = indices_used.clone() {
                meta.insert("indices_used".into(), json!(indices));
            }
            if let Some(hash) = query_hash.clone() {
                meta.insert("query_hash".into(), json!(hash));
            }
            Value::Object(meta)
        },
    }
}

pub fn new_plan_id(_anchor: &Anchor) -> String {
    format!("llm-plan-{}", Uuid::now_v7())
}
