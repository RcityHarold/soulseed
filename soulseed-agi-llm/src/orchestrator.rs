use crate::dto::{LlmInput, LlmPlan, LlmResult, ModelProfile, PromptBundle};
use crate::errors::EngineError;
use crate::tw_client::ThinWaistClient;

#[derive(Clone, Debug)]
pub struct ExecutionOutcome {
    pub result: LlmResult,
    pub degradation_chain: Option<String>,
    pub indices_used: Option<Vec<String>>,
    pub query_hash: Option<String>,
}

#[derive(Clone, Default)]
pub struct LlmOrchestrator;

impl LlmOrchestrator {
    #[allow(clippy::too_many_arguments)]
    pub fn execute<C: ThinWaistClient>(
        &self,
        client: &C,
        input: &LlmInput,
        plan: &LlmPlan,
        model: &ModelProfile,
        prompt: &PromptBundle,
    ) -> Result<ExecutionOutcome, EngineError> {
        let _ = plan;
        let result = client.execute(&input.anchor, prompt, model)?;
        client.reconcile(&input.anchor, &result)?;

        let degradation_chain = match (
            input.degrade_hint.as_ref(),
            result.degradation_reason.as_ref(),
        ) {
            (Some(tool), Some(llm)) => Some(format!("{}|{}", tool, llm)),
            (Some(tool), None) => Some(tool.clone()),
            (None, Some(llm)) => Some(llm.clone()),
            (None, None) => None,
        };

        let mut merged_indices = input.tool_indices.clone().unwrap_or_default();
        if let Some(llm_indices) = result.indices_used.clone() {
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

        let query_hash = match (input.tool_query_hash.clone(), result.query_hash.clone()) {
            (Some(tool), Some(llm)) if tool != llm => Some(format!("{}|{}", tool, llm)),
            (Some(tool), Some(_)) => Some(tool),
            (Some(tool), None) => Some(tool),
            (None, Some(llm)) => Some(llm),
            (None, None) => None,
        };

        Ok(ExecutionOutcome {
            result,
            degradation_chain,
            indices_used,
            query_hash,
        })
    }
}
