use crate::dto::LlmInput;
use crate::errors::EngineError;
use serde_json::Value;
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, DecisionPlan,
};
use soulseed_agi_dfr::types::RouterDecision;
use soulseed_agi_tools::dto::{Anchor, ToolResultSummary};

#[derive(Clone, Debug)]
pub struct LlmIntegrationOptions {
    pub user_prompt: String,
    pub context_tags: Value,
    pub clarify_prompt: Option<String>,
    pub tool_summary: Option<ToolResultSummary>,
    pub tool_indices: Option<Vec<String>>,
    pub tool_query_hash: Option<String>,
}

impl LlmIntegrationOptions {
    pub fn new(user_prompt: String) -> Self {
        Self {
            user_prompt,
            context_tags: Value::Null,
            clarify_prompt: None,
            tool_summary: None,
            tool_indices: None,
            tool_query_hash: None,
        }
    }

    pub fn context_tags(mut self, tags: Value) -> Self {
        self.context_tags = tags;
        self
    }

    pub fn clarify_prompt(mut self, prompt: Option<String>) -> Self {
        self.clarify_prompt = prompt;
        self
    }

    pub fn tool_summary(mut self, summary: Option<ToolResultSummary>) -> Self {
        self.tool_summary = summary;
        self
    }

    pub fn tool_indices(mut self, indices: Option<Vec<String>>) -> Self {
        self.tool_indices = indices;
        self
    }

    pub fn tool_query_hash(mut self, hash: Option<String>) -> Self {
        self.tool_query_hash = hash;
        self
    }
}

pub fn build_llm_input(
    decision: &RouterDecision,
    opts: LlmIntegrationOptions,
) -> Result<LlmInput, EngineError> {
    let mut indices = decision.plan.explain.indices_used.clone();
    if let Some(extra) = opts.tool_indices.as_ref() {
        for idx in extra {
            if !indices.iter().any(|existing| existing == idx) {
                indices.push(idx.clone());
            }
        }
    }
    let tool_indices = if indices.is_empty() {
        None
    } else {
        Some(indices)
    };

    let tool_query_hash = match (
        decision.plan.explain.query_hash.clone(),
        opts.tool_query_hash,
    ) {
        (Some(base), Some(extra)) if base != extra => Some(format!("{}|{}", base, extra)),
        (Some(base), Some(_)) => Some(base),
        (Some(base), None) => Some(base),
        (None, Some(extra)) => Some(extra),
        (None, None) => None,
    };

    let degrade_hint = decision
        .plan
        .explain
        .degradation_reason
        .clone()
        .or_else(|| {
            decision
                .decision_path
                .degradation_reason
                .map(map_awareness_degradation)
                .map(|s| s.to_string())
        });

    let scene = match &decision.plan.decision_plan {
        DecisionPlan::Clarify { .. } => "clarify_llm",
        DecisionPlan::Tool { .. } => "tool_llm",
        DecisionPlan::Collab { .. } => "collab_llm",
        DecisionPlan::SelfReason { .. } => "self_reason_llm",
    };

    Ok(LlmInput {
        anchor: convert_anchor(&decision.plan.anchor),
        scene: scene.into(),
        clarify_prompt: opts.clarify_prompt,
        tool_summary: opts.tool_summary,
        user_prompt: opts.user_prompt,
        context_tags: opts.context_tags,
        degrade_hint,
        tool_indices,
        tool_query_hash,
    })
}

fn convert_anchor(anchor: &AwarenessAnchor) -> Anchor {
    Anchor {
        tenant_id: anchor.tenant_id,
        envelope_id: anchor.envelope_id,
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id,
        sequence_number: anchor.sequence_number,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        schema_v: anchor.schema_v,
        scenario: None,
    }
}

fn map_awareness_degradation(reason: AwarenessDegradationReason) -> &'static str {
    match reason {
        AwarenessDegradationReason::BudgetTokens => "budget_tokens",
        AwarenessDegradationReason::BudgetWalltime => "budget_walltime",
        AwarenessDegradationReason::BudgetExternalCost => "budget_external_cost",
        AwarenessDegradationReason::EmptyCatalog => "empty_catalog",
        AwarenessDegradationReason::PrivacyBlocked => "privacy_blocked",
        AwarenessDegradationReason::InvalidPlan => "invalid_plan",
        AwarenessDegradationReason::ClarifyExhausted => "clarify_exhausted",
        AwarenessDegradationReason::GraphDegraded => "graph_degraded",
        AwarenessDegradationReason::EnvctxDegraded => "envctx_degraded",
    }
}
