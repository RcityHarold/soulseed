use crate::dto::{
    ExecutionContext, LlmExecutionIntent, LlmInput, LlmPlan, LlmResult, ModelProfile, PromptBundle,
    ReasoningVisibility,
};
use crate::errors::EngineError;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LlmOrchestrator;

impl LlmOrchestrator {
    pub fn prepare_intent(
        &self,
        input: &LlmInput,
        plan: &LlmPlan,
        model: &ModelProfile,
        prompt: &PromptBundle,
    ) -> LlmExecutionIntent {
        LlmExecutionIntent {
            intent_id: format!("llm-intent-{}", Uuid::now_v7()),
            anchor: plan.anchor.clone(),
            schema_v: plan.schema_v,
            lineage: plan.lineage.clone(),
            model: model.clone(),
            prompt: prompt.clone(),
            privacy: input.privacy.clone(),
            degrade_hint: input.degrade_hint.clone(),
            tool_indices: input.tool_indices.clone(),
            tool_query_hash: input.tool_query_hash.clone(),
            issued_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn compose_context(
        &self,
        intent: &LlmExecutionIntent,
        mut result: LlmResult,
    ) -> Result<ExecutionContext, EngineError> {
        if result.evidence_pointer.is_none() {
            return Err(EngineError::MissingEvidencePointer);
        }

        let ticket_provided = intent
            .privacy
            .ticket_id
            .as_ref()
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false);
        let allow_sensitive = intent.privacy.allow_sensitive || ticket_provided;
        let can_keep_full = match result.reasoning_visibility {
            ReasoningVisibility::Full => allow_sensitive,
            ReasoningVisibility::TicketRequired => ticket_provided,
            _ => false,
        };

        if !can_keep_full {
            if result.summary.is_none() {
                result.summary = Some(result.completion.clone());
            }
            result.completion = result.summary.clone().unwrap_or_default();
            result.reasoning.clear();
            result.reasoning_visibility = ReasoningVisibility::SummaryOnly;
            result.redacted = true;
        } else {
            result.redacted = false;
        }

        let degradation_chain = match (
            intent.degrade_hint.as_ref(),
            result.degradation_reason.as_ref(),
        ) {
            (Some(tool), Some(llm)) if tool != llm => Some(format!("{tool}|{llm}")),
            (Some(tool), _) => Some(tool.clone()),
            (None, Some(llm)) => Some(llm.clone()),
            _ => None,
        };

        let mut merged_indices = intent.tool_indices.clone().unwrap_or_default();
        if let Some(llm_indices) = result.indices_used.take() {
            for idx in llm_indices {
                if !merged_indices.iter().any(|existing| existing == &idx) {
                    merged_indices.push(idx);
                }
            }
        }
        let indices_used = if merged_indices.is_empty() {
            None
        } else {
            Some(merged_indices)
        };

        let query_hash = match (intent.tool_query_hash.clone(), result.query_hash.take()) {
            (Some(tool), Some(llm)) if tool != llm => Some(format!("{tool}|{llm}")),
            (Some(tool), Some(_)) => Some(tool),
            (Some(tool), None) => Some(tool),
            (None, Some(llm)) => Some(llm),
            _ => None,
        };

        result.lineage = intent.lineage.clone();

        Ok(ExecutionContext {
            intent: intent.clone(),
            result,
            degradation_chain,
            indices_used,
            query_hash,
        })
    }
}
